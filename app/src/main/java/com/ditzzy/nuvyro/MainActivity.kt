package com.ditzzy.nuvyro

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.ditzzy.nuvyro.ui.home.HomeScreen
import com.ditzzy.nuvyro.ui.theme.NuvyroTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            NuvyroTheme {
                HomeScreen()
            }
        }
    }
}
