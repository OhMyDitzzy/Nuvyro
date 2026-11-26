package com.ditzzy.nuvyro.data.setup

import android.content.Context
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class SetupPreferences(context: Context) {
    private val preferences = context.applicationContext
        .getSharedPreferences("setup", Context.MODE_PRIVATE)

    suspend fun load(): SetupProgress = withContext(Dispatchers.IO) {
        SetupProgress(
            step = SetupStep.entries.firstOrNull {
                it.name == preferences.getString("step", null)
            } ?: SetupStep.Welcome,
            receiveFolderUri = preferences.getString("receive_folder_uri", null),
            completed = preferences.getBoolean("completed", false)
        )
    }

    suspend fun save(progress: SetupProgress) = withContext(Dispatchers.IO) {
        val saved = preferences.edit()
            .putString("step", progress.step.name)
            .putString("receive_folder_uri", progress.receiveFolderUri)
            .putBoolean("completed", progress.completed)
            .commit()
        if (!saved) throw IOException("Could not save setup progress")
    }
}
