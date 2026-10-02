/// Auth handler — manages biometric prompts and response signing.
///
/// When an auth request arrives from the laptop:
/// 1. Show a biometric prompt (fingerprint / face unlock)
/// 2. If verified, sign the response with the pairing key
/// 3. Send the signed response back to the laptop

import 'package:flutter/foundation.dart';
import 'package:local_auth/local_auth.dart';
import 'package:local_auth/android_auth_messages.dart';

import 'model/auth_request.dart';

class AuthHandler extends ChangeNotifier {
  final LocalAuthentication _localAuth = LocalAuthentication();

  AuthRequest? _pendingRequest;
  AuthRequest? get pendingRequest => _pendingRequest;

  bool _isProcessing = false;
  bool get isProcessing => _isProcessing;

  String? _lastResult;
  String? get lastResult => _lastResult;

  /// Check if biometric auth is available on this device.
  Future<bool> isBiometricAvailable() async {
    try {
      return await _localAuth.isDeviceSupported();
    } catch (e) {
      debugPrint('Biometric check failed: $e');
      return false;
    }
  }

  /// Process an incoming auth request.
  ///
  /// Shows a biometric prompt and signs the response if verified.
  Future<void> processRequest(AuthRequest request) async {
    if (_isProcessing) {
      debugPrint('Already processing a request');
      return;
    }

    _isProcessing = true;
    _pendingRequest = request;
    _lastResult = null;
    notifyListeners();

    try {
      // Show biometric prompt
      final authenticated = await _localAuth.authenticate(
        localizedReason: 'Authenticate to authorize sudo for "$request.username"',
        options: const AuthenticationOptions(
          biometricOnly: true,
          stickyAuth: true,
        ),
      );

      if (authenticated) {
        // Sign the response with the pairing key
        // (In production, use the stored pairing key)
        final signature = _signResponse(request);

        final response = AuthResponse(
          nonce: request.nonce,
          username: request.username,
          signature: signature,
        );

        _lastResult = 'Authenticated — response sent';
        debugPrint('Auth response: ${response.encode().length} bytes');

        // Send response via BLE
        // (The BleClient would handle this)
      } else {
        _lastResult = 'Authentication failed or cancelled';
        debugPrint('User not authenticated');
      }
    } catch (e) {
      _lastResult = 'Error: $e';
      debugPrint('Auth error: $e');
    } finally {
      _isProcessing = false;
      _pendingRequest = null;
      notifyListeners();
    }
  }

  /// Sign an auth response with the pairing key.
  ///
  /// In production, this uses the stored P-256 pairing key.
  /// For now, returns a dummy signature.
  List<int> _signResponse(AuthRequest request) {
    // TODO: Implement actual signing with pairing key
    // HMAC-SHA256(pairingKey, nonce || username || timestamp)
    // For now, return a dummy signature
    return List.filled(32, 0);
  }
}
