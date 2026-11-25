package com.ditzzy.nuvyro.ui.theme

import android.content.res.Configuration
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import androidx.compose.ui.unit.dp

class ThemeColorPreviewProvider : PreviewParameterProvider<ThemeColor> {
    override val values: Sequence<ThemeColor> = ThemeColor.entries.asSequence()
}

@Preview(name = "Light", uiMode = Configuration.UI_MODE_NIGHT_NO, widthDp = 360)
@Preview(name = "Dark", uiMode = Configuration.UI_MODE_NIGHT_YES, widthDp = 360)
@Composable
private fun NuvyroThemePreview(
    @PreviewParameter(ThemeColorPreviewProvider::class) themeColor: ThemeColor
) {
    NuvyroTheme(themeColor = themeColor) {
        Surface {
            Column(
                modifier = Modifier.padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                Text(
                    text = when (themeColor) {
                        ThemeColor.Blue -> "Blue"
                        ThemeColor.Red -> "Red"
                        ThemeColor.Yellow -> "Yellow"
                        ThemeColor.Orange -> "Orange"
                    },
                    style = MaterialTheme.typography.headlineMedium
                )
                Text(
                    text = "Material 3 Expressive theme",
                    style = MaterialTheme.typography.bodyLarge
                )
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    color = MaterialTheme.colorScheme.primaryContainer,
                    contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                    shape = MaterialTheme.shapes.large
                ) {
                    Text("Primary color", modifier = Modifier.padding(20.dp))
                }
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    color = MaterialTheme.colorScheme.tertiaryContainer,
                    contentColor = MaterialTheme.colorScheme.onTertiaryContainer,
                    shape = MaterialTheme.shapes.large
                ) {
                    Text("Tertiary color", modifier = Modifier.padding(20.dp))
                }
                Button(onClick = {}) {
                    Text("Primary button")
                }
                FilledTonalButton(onClick = {}) {
                    Text("Secondary button")
                }
            }
        }
    }
}
