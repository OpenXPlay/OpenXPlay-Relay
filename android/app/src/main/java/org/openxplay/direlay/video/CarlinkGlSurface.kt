// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.video

import android.graphics.SurfaceTexture
import android.opengl.EGL14
import android.opengl.EGLExt
import android.opengl.GLES11Ext
import android.opengl.GLES20
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Owned by the codec worker; the callback thread only signals that a decoder image is ready. */
internal class CarlinkGlSurface(encoder: Surface) : AutoCloseable {
    private val display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)
    private var context = EGL14.EGL_NO_CONTEXT
    private var window = EGL14.EGL_NO_SURFACE
    private var texture = 0
    private var program = 0
    private var image: SurfaceTexture? = null
    private var decoderSurface: Surface? = null
    private val callbacks = HandlerThread("relay-video-images")
    private val gate = Object()
    private var available = false
    private val transform = FloatArray(16)
    val surface: Surface get() = checkNotNull(decoderSurface)

    init {
        try {
            check(display != EGL14.EGL_NO_DISPLAY && EGL14.eglInitialize(display, IntArray(2), 0, IntArray(2), 0))
            val configs = arrayOfNulls<android.opengl.EGLConfig>(1)
            val count = IntArray(1)
            check(EGL14.eglChooseConfig(display, intArrayOf(EGL14.EGL_RED_SIZE, 8, EGL14.EGL_GREEN_SIZE, 8,
                EGL14.EGL_BLUE_SIZE, 8, EGL14.EGL_RENDERABLE_TYPE, EGL14.EGL_OPENGL_ES2_BIT,
                0x3142, 1, EGL14.EGL_NONE), 0, configs, 0, 1, count, 0) && count[0] > 0)
            context = EGL14.eglCreateContext(display, configs[0], EGL14.EGL_NO_CONTEXT,
                intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 2, EGL14.EGL_NONE), 0)
            check(context != EGL14.EGL_NO_CONTEXT)
            window = EGL14.eglCreateWindowSurface(display, configs[0], encoder, intArrayOf(EGL14.EGL_NONE), 0)
            check(window != EGL14.EGL_NO_SURFACE && EGL14.eglMakeCurrent(display, window, window, context))
            val ids = IntArray(1)
            GLES20.glGenTextures(1, ids, 0); texture = ids[0]
            GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, texture)
            GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR)
            GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR)
            GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE)
            GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE)
            program = GLES20.glCreateProgram()
            val vertex = shader(GLES20.GL_VERTEX_SHADER, "attribute vec2 position; attribute vec2 uv; uniform mat4 transform; varying vec2 tex; void main(){ gl_Position=vec4(position,0.0,1.0); tex=(transform*vec4(uv,0.0,1.0)).xy; }")
            val fragment = shader(GLES20.GL_FRAGMENT_SHADER, "#extension GL_OES_EGL_image_external : require\nprecision mediump float; uniform samplerExternalOES image; varying vec2 tex; void main(){ gl_FragColor=texture2D(image,tex); }")
            try {
                GLES20.glAttachShader(program, vertex); GLES20.glAttachShader(program, fragment)
                GLES20.glLinkProgram(program)
                GLES20.glGetProgramiv(program, GLES20.GL_LINK_STATUS, ids, 0)
                check(ids[0] != 0) { GLES20.glGetProgramInfoLog(program) }
            } finally { GLES20.glDeleteShader(vertex); GLES20.glDeleteShader(fragment) }
            callbacks.start()
            image = SurfaceTexture(texture).also { st ->
                st.setOnFrameAvailableListener({ synchronized(gate) { available = true; gate.notifyAll() } }, Handler(callbacks.looper))
                decoderSurface = Surface(st)
            }
        } catch (failure: Exception) { close(); throw failure }
    }

    fun draw(geometry: CarlinkGeometry, timestamp: Long, running: () -> Boolean) {
        val deadline = System.nanoTime() + 1_000_000_000L
        synchronized(gate) {
            while (!available && running() && System.nanoTime() < deadline) gate.wait(10)
            if (!running()) return
            check(available) { "decoder_surface_timeout" }
            available = false
        }
        val st = checkNotNull(image)
        st.updateTexImage(); st.getTransformMatrix(transform)
        val a = geometry.area
        val left = a.x.toFloat() / geometry.canvas.width
        val right = (a.x + a.width).toFloat() / geometry.canvas.width
        val bottom = 1f - (a.y + a.height).toFloat() / geometry.canvas.height
        val top = 1f - a.y.toFloat() / geometry.canvas.height
        val vertices = floats(floatArrayOf(-1f, -1f, 1f, -1f, -1f, 1f, 1f, 1f))
        val coordinates = floats(floatArrayOf(left, bottom, right, bottom, left, top, right, top))
        GLES20.glViewport(0, 0, geometry.picture.width, geometry.picture.height)
        GLES20.glUseProgram(program)
        val position = GLES20.glGetAttribLocation(program, "position")
        val uv = GLES20.glGetAttribLocation(program, "uv")
        GLES20.glEnableVertexAttribArray(position); GLES20.glEnableVertexAttribArray(uv)
        GLES20.glVertexAttribPointer(position, 2, GLES20.GL_FLOAT, false, 0, vertices)
        GLES20.glVertexAttribPointer(uv, 2, GLES20.GL_FLOAT, false, 0, coordinates)
        GLES20.glUniformMatrix4fv(GLES20.glGetUniformLocation(program, "transform"), 1, false, transform, 0)
        GLES20.glActiveTexture(GLES20.GL_TEXTURE0)
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, texture)
        GLES20.glUniform1i(GLES20.glGetUniformLocation(program, "image"), 0)
        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4)
        check(GLES20.glGetError() == GLES20.GL_NO_ERROR) { "crop_gl_failed" }
        check(EGLExt.eglPresentationTimeANDROID(display, window, timestamp))
        check(EGL14.eglSwapBuffers(display, window)) { "crop_swap_failed" }
    }

    override fun close() {
        decoderSurface?.release(); decoderSurface = null
        image?.release(); image = null
        callbacks.quitSafely()
        if (display != EGL14.EGL_NO_DISPLAY) {
            if (context != EGL14.EGL_NO_CONTEXT) {
                if (program != 0) GLES20.glDeleteProgram(program)
                if (texture != 0) GLES20.glDeleteTextures(1, intArrayOf(texture), 0)
                EGL14.eglMakeCurrent(display, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT)
            }
            if (window != EGL14.EGL_NO_SURFACE) EGL14.eglDestroySurface(display, window)
            if (context != EGL14.EGL_NO_CONTEXT) EGL14.eglDestroyContext(display, context)
            EGL14.eglReleaseThread(); EGL14.eglTerminate(display)
        }
        window = EGL14.EGL_NO_SURFACE; context = EGL14.EGL_NO_CONTEXT
    }

    private fun shader(kind: Int, source: String): Int {
        val id = GLES20.glCreateShader(kind)
        GLES20.glShaderSource(id, source); GLES20.glCompileShader(id)
        val status = IntArray(1)
        GLES20.glGetShaderiv(id, GLES20.GL_COMPILE_STATUS, status, 0)
        if (status[0] == 0) {
            val error = GLES20.glGetShaderInfoLog(id)
            GLES20.glDeleteShader(id); error(error)
        }
        return id
    }
    private fun floats(values: FloatArray) = ByteBuffer.allocateDirect(values.size * 4)
        .order(ByteOrder.nativeOrder()).asFloatBuffer().apply { put(values); position(0) }
}
