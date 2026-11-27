# Add project specific ProGuard rules here.

-keep class com.ditzzy.nuvyro.core.transfer.NativeTransferBridge { *; }
-keep class com.ditzzy.nuvyro.core.transfer.NativeTransferException {
    public <init>(int);
}
