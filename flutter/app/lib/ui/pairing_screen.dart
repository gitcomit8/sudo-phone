/// Pairing screen — connects to the laptop and exchanges keys.

import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../ble/ble_client.dart';

class PairingScreen extends StatefulWidget {
  const PairingScreen({super.key});

  @override
  State<PairingScreen> createState() => _PairingScreenState();
}

class _PairingScreenState extends State<PairingScreen> {
  bool _scanning = false;
  String? _error;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Pair with Laptop'),
      ),
      body: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text(
              'Pairing establishes a shared key between this phone and your laptop.',
              style: TextStyle(fontSize: 16),
            ),
            const SizedBox(height: 24),
            ElevatedButton.icon(
              onPressed: _scanning ? null : _startPairing,
              icon: _scanning
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.bluetooth_searching),
              label: Text(_scanning ? 'Scanning...' : 'Scan for Laptop'),
            ),
            if (_error != null) ...[
              const SizedBox(height: 16),
              Card(
                color: Colors.red.shade50,
                child: Padding(
                  padding: const EdgeInsets.all(12.0),
                  child: Text(_error!, style: const TextStyle(color: Colors.red)),
                ),
              ),
            ],
            const SizedBox(height: 24),
            Consumer<BleClient>(
              builder: (_, client, __) {
                if (client.connected) {
                  return Card(
                    color: Colors.green.shade50,
                    child: Padding(
                      padding: const EdgeInsets.all(16.0),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          const Text(
                            'Pairing successful!',
                            style: TextStyle(
                              fontWeight: FontWeight.bold,
                              color: Colors.green,
                            ),
                          ),
                          const SizedBox(height: 8),
                          Text('Laptop: ${client.laptop?.platformName ?? 'Unknown'}'),
                        ],
                      ),
                    ),
                  );
                }
                return const SizedBox.shrink();
              },
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _startPairing() async {
    setState(() {
      _scanning = true;
      _error = null;
    });

    try {
      final success = await context.read<BleClient>().connectToLaptop();
      if (mounted) {
        setState(() {
          _scanning = false;
          if (!success) {
            _error = 'No laptop found. Make sure it\'s in pairing mode.';
          }
        });
        if (success) {
          Navigator.pop(context, true);
        }
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _scanning = false;
          _error = 'Pairing failed: $e';
        });
      }
    }
  }
}
