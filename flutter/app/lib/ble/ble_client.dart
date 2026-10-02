/// BLE client — communicates with the laptop daemon over BLE.
///
/// Connects to the laptop's CTAP2 GATT service and sends/receives
/// auth requests and responses.

import 'package:flutter/foundation.dart';
import 'package:flutter_blue_plus/flutter_blue_plus.dart';

import '../model/auth_request.dart';

class BleClient extends ChangeNotifier {
  FlutterBluePlus get bluetooth => FlutterBluePlus.instance;

  BluetoothDevice? _laptop;
  BluetoothCharacteristic? _requestChar; // Laptop writes auth requests here
  BluetoothCharacteristic? _responseChar; // Phone writes responses here

  bool _connected = false;
  bool get connected => _connected;

  BluetoothDevice? get laptop => _laptop;

  /// Scan for and connect to the laptop daemon.
  ///
  /// The laptop advertises a BLE service with the FIDO2 UUID:
  ///   0000fee0-0000-1000-8000-00805f9b34fb
  Future<bool> connectToLaptop() async {
    // Start scanning for the FIDO2 service
    final results = await bluetooth.startScan(
      timeout: const Duration(seconds: 10),
      withServices: [Guid('0000fee0-0000-1000-8000-00805f9b34fb')],
    );

    if (results.isEmpty) {
      debugPrint('No laptop found advertising FIDO2 service');
      return false;
    }

    // Connect to the first device found
    _laptop = results.first.device;
    await _laptop!.connect();
    debugPrint('Connected to laptop: ${_laptop!.platformName}');

    // Discover services
    await _discoverGattServices();

    _connected = true;
    notifyListeners();
    return true;
  }

  /// Discover the CTAP2 request and response characteristics.
  Future<void> _discoverGattServices() async {
    if (_laptop == null) return;

    final services = await _laptop!.discoverServices();

    for (final service in services) {
      if (service.uuid.uuid != '0000fee0-0000-1000-8000-00805f9b34fb') {
        continue;
      }

      for (final char in service.characteristics) {
        switch (char.uuid.uuid) {
          case '00000001-0000-1000-8000-00805f9b34fb':
            // Request characteristic — laptop writes auth requests
            await char.setNotifyValue(true);
            char.value.listen((value) {
              _handleAuthRequest(value);
            });
            _requestChar = char;
            debugPrint('Found CTAP2 request characteristic');
            break;

          case '00000002-0000-1000-8000-00805f9b34fb':
            // Response characteristic — phone writes responses
            _responseChar = char;
            debugPrint('Found CTAP2 response characteristic');
            break;
        }
      }
    }
  }

  /// Handle an incoming auth request from the laptop.
  void _handleAuthRequest(List<int> value) {
    // Parse the CTAP2 auth request
    // Format: [cmd=0x06, nonce(8 bytes), username..., ttl(4 bytes)]
    if (value.isEmpty || value[0] != 0x06) {
      debugPrint('Invalid auth request');
      return;
    }

    if (value.length < 12) {
      debugPrint('Auth request too short');
      return;
    }

    final nonceBytes = value.sublist(1, 9);
    final nonce = _bytesToUint64(nonceBytes);
    final username = String.fromCharCodes(value.sublist(9, value.length - 4));
    final ttlBytes = value.sublist(value.length - 4, value.length);
    final ttl = _bytesToUint64(ttlBytes);

    final request = AuthRequest(
      nonce: nonce,
      username: username,
      ttl: ttl,
    );

    debugPrint('Auth request: nonce=${request.nonce.toRadixString(16)}, user=$username');

    // Notify the AuthHandler to process this request
    // (The AuthHandler will trigger biometric prompt)
    // This is handled via the provider system
  }

  /// Send an auth response back to the laptop.
  Future<void> sendResponse(AuthResponse response) async {
    if (_requestChar == null) {
      throw StateError('Not connected to laptop');
    }

    // Encode the response as CBOR
    // Format: [cmd=0x06, nonce(8 bytes), signature...]
    final encoded = response.encode();
    await _requestChar!.write(encoded);
    debugPrint('Sent auth response (${encoded.length} bytes)');
  }

  /// Disconnect from the laptop.
  Future<void> disconnect() async {
    if (_laptop != null) {
      await _laptop!.disconnect();
      _connected = false;
      _laptop = null;
      _requestChar = null;
      _responseChar = null;
      notifyListeners();
    }
  }

  /// Convert 8 bytes to int (little-endian, unsigned interpretation)
  int _bytesToUint64(List<int> bytes) {
    int result = 0;
    for (int i = 0; i < 8 && i < bytes.length; i++) {
      result |= (bytes[i] & 0xFF) << (i * 8);
    }
    return result;
  }

  @override
  void dispose() {
    disconnect();
    super.dispose();
  }
}
