import { useState } from "react";
import { Button, StyleSheet, Text, View } from "react-native";
import { CameraView, useCameraPermissions, type BarcodeScanningResult } from "expo-camera";
import { router } from "expo-router";
import { parsePairPayload } from "../src/lib/pairing";
import { pairUrl } from "../src/lib/api";
import { saveSession } from "../src/store/session";

export default function Pair() {
  const [permission, requestPermission] = useCameraPermissions();
  const [scanned, setScanned] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pairing, setPairing] = useState(false);

  async function handleScan({ data }: BarcodeScanningResult) {
    if (scanned || pairing) return;
    setScanned(true);
    setError(null);

    const parsed = parsePairPayload(data);
    if (!parsed) {
      setError("Invalid QR code");
      setScanned(false);
      return;
    }

    setPairing(true);
    try {
      const res = await fetch(pairUrl(parsed.baseUrl, parsed.code));
      if (!res.ok) throw new Error(`pair failed (${res.status})`);
      const body = await res.json();
      if (!body || typeof body.token !== "string") throw new Error("pair failed (no token)");
      await saveSession(parsed.baseUrl, body.token);
      router.replace("/");
    } catch {
      setError("Pairing failed. Please try again.");
      setScanned(false);
    } finally {
      setPairing(false);
    }
  }

  if (!permission) {
    return <View style={styles.container} />;
  }

  if (!permission.granted) {
    return (
      <View style={styles.container}>
        <Text style={styles.text}>Camera permission is required to scan the pairing QR code.</Text>
        <Button title="Grant permission" onPress={requestPermission} />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
        onBarcodeScanned={scanned ? undefined : handleScan}
      />
      {error ? (
        <View style={styles.overlay}>
          <Text style={styles.errorText}>{error}</Text>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
  },
  text: {
    textAlign: "center",
    marginBottom: 12,
  },
  overlay: {
    position: "absolute",
    bottom: 40,
    left: 20,
    right: 20,
    backgroundColor: "rgba(0,0,0,0.7)",
    borderRadius: 8,
    padding: 12,
  },
  errorText: {
    color: "white",
    textAlign: "center",
  },
});
