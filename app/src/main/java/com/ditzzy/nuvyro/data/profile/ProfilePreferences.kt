package com.ditzzy.nuvyro.data.profile

import android.content.Context
import android.os.Build
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class ProfilePreferences(context: Context) {
    private val preferences = context.applicationContext
        .getSharedPreferences("profile", Context.MODE_PRIVATE)

    suspend fun load(): LocalProfile = withContext(Dispatchers.IO) {
        val name = preferences.getString("display_name", null)
            ?.let(ProfileName::normalize)?.takeIf(ProfileName::isValid)
            ?: ProfileName.deviceDefault(Build.MODEL)
        LocalProfile(name, preferences.getString("photo_file_name", null))
    }

    suspend fun save(profile: LocalProfile) = withContext(Dispatchers.IO) {
        require(ProfileName.isValid(profile.displayName))
        val saved = preferences.edit()
            .putString("display_name", ProfileName.normalize(profile.displayName))
            .putString("photo_file_name", profile.photoFileName)
            .commit()
        if (!saved) throw IOException("Could not save profile")
    }
}
