package com.ditzzy.nuvyro.core.lan.profile

import com.ditzzy.nuvyro.data.profile.ProfileName
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.util.UUID

internal data class ProfilePacket(val sessionId: String, val name: String, val photo: ByteArray)

internal object ProfileProtocol {
    const val ServiceType = "_nuvyro._tcp."
    const val Magic = 0x4e565031
    const val Version = 1
    const val MaxPhotoBytes = 32 * 1024
    const val HotspotPort = 47738

    fun write(output: DataOutputStream, packet: ProfilePacket) {
        output.writeInt(Magic)
        output.writeInt(Version)
        output.writeText(packet.sessionId)
        output.writeText(packet.name)
        output.writeInt(packet.photo.size)
        output.write(packet.photo)
        output.flush()
    }

    fun read(input: DataInputStream): ProfilePacket {
        if (input.readInt() != Magic || input.readInt() != Version) throw IOException("Unknown profile protocol")
        val id = input.readText(36)
        if (UUID.fromString(id).toString() != id) throw IOException("Invalid session ID")
        val name = input.readText(128)
        if (!ProfileName.isValid(name)) throw IOException("Invalid profile name")
        val size = input.readInt()
        if (size !in 0..MaxPhotoBytes) throw IOException("Profile photo is too large")
        val photo = ByteArray(size)
        input.readFully(photo)
        return ProfilePacket(id, ProfileName.normalize(name), photo)
    }

    private fun DataOutputStream.writeText(text: String) {
        val bytes = text.toByteArray(Charsets.UTF_8)
        writeShort(bytes.size)
        write(bytes)
    }

    private fun DataInputStream.readText(max: Int): String {
        val size = readUnsignedShort()
        if (size !in 1..max) throw IOException("Invalid text length")
        val bytes = ByteArray(size)
        readFully(bytes)
        return Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
    }
}
