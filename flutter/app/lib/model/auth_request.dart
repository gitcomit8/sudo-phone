/// Data models for auth requests and responses.

/// An auth request from the laptop daemon.
class AuthRequest {
  final int nonce;
  final String username;
  final int ttl; // seconds

  AuthRequest({
    required this.nonce,
    required this.username,
    required this.ttl,
  });

  @override
  String toString() =>
      'AuthRequest(nonce=${nonce.toRadixString(16)}, user=$username, ttl=$ttl)s';
}

/// A signed auth response sent back to the laptop.
class AuthResponse {
  final int nonce;
  final String username;
  final List<int> signature; // HMAC-SHA256 or ECDSA signature

  AuthResponse({
    required this.nonce,
    required this.username,
    required this.signature,
  });

  /// Encode as CBOR-like bytes: [cmd=0x06, nonce(8), signature...]
  List<int> encode() {
    final nonceBytes = _uint64ToBytes(nonce);
    return [0x06, ...nonceBytes, ...signature];
  }

  /// Convert int to 8 bytes (little-endian)
  List<int> _uint64ToBytes(int value) {
    final bytes = <int>[];
    for (int i = 0; i < 8; i++) {
      bytes.add((value >> (i * 8)) & 0xFF);
    }
    return bytes;
  }
}
