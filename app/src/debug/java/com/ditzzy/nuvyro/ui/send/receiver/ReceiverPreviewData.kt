package com.ditzzy.nuvyro.ui.send.receiver

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint

internal val previewReceivers = listOf(
    ReceiverItem("alex", "Alex", portrait(Color.rgb(219, 227, 255), Color.rgb(49, 66, 130))),
    ReceiverItem("maya", "Maya", portrait(Color.rgb(251, 222, 203), Color.rgb(140, 73, 44))),
    ReceiverItem("bima", "Bima"),
    ReceiverItem("long-name", "A receiver with a longer profile name"),
    ReceiverItem("rani", "Rani"),
    ReceiverItem("android", "Android device"),
    ReceiverItem("sam", "Sam")
)

private fun portrait(background: Int, shirt: Int): Bitmap =
    Bitmap.createBitmap(96, 96, Bitmap.Config.ARGB_8888).apply {
        val canvas = Canvas(this)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        canvas.drawColor(background)
        paint.color = shirt
        canvas.drawRoundRect(12f, 55f, 84f, 112f, 30f, 30f, paint)
        paint.color = Color.rgb(192, 136, 100)
        canvas.drawCircle(48f, 36f, 20f, paint)
    }
