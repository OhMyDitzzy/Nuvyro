package com.ditzzy.nuvyro.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.runtime.Composable

/** Follows system brightness unless overridden, with blue as the default accent. */
@Composable
fun NuvyroTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    themeColor: ThemeColor = ThemeColor.Blue,
    content: @Composable () -> Unit
) {
    MaterialExpressiveTheme(
        colorScheme = themeColor.colorScheme(darkTheme),
        typography = Typography,
        content = content
    )
}
