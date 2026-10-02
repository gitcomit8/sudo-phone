/// Home screen — shows connection status and pairing controls.

import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../ble/ble_client.dart';
import '../auth_handler.dart';
import 'pairing_screen.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Phone Auth'),
        actions: [
          Consumer<BleClient>(
            builder: (_, client, __) {
              if (client.connected) {
                return IconButton(
                  icon: const Icon(Icons.link, color: Colors.green),
                  tooltip: 'Connected to laptop',
                  onPressed: () async {
                    await client.disconnect();
                  },
                );
              }
              return const SizedBox.shrink();
            },
          ),
        ],
      ),
      body: const _HomeBody(),
    );
  }
}

class _HomeBody extends StatelessWidget {
  const _HomeBody();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _ConnectionStatus(),
          const SizedBox(height: 24),
          ElevatedButton.icon(
            onPressed: () async {
              final result = await Navigator.push(
                context,
                MaterialPageRoute(builder: (_) => const PairingScreen()),
              );
              if (result == true) {
                // Re-scan after pairing
                context.read<BleClient>().connectToLaptop();
              }
            },
            icon: const Icon(Icons.bluetooth_searching),
            label: const Text('Pair with Laptop'),
          ),
          const SizedBox(height: 12),
          Consumer<AuthHandler>(
            builder: (_, handler, __) {
              if (handler.lastResult != null) {
                return Card(
                  color: handler.lastResult!.startsWith('Error')
                      ? Colors.red.shade50
                      : handler.lastResult == 'Authenticated'
                          ? Colors.green.shade50
                          : null,
                  child: Padding(
                    padding: const EdgeInsets.all(16.0),
                    child: Text(
                      handler.lastResult!,
                      style: TextStyle(
                        color: handler.lastResult!.startsWith('Error')
                            ? Colors.red
                            : handler.lastResult == 'Authenticated'
                                ? Colors.green
                                : null,
                      ),
                    ),
                  ),
                );
              }
              return const SizedBox.shrink();
            },
          ),
        ],
      ),
    );
  }
}

class _ConnectionStatus extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Consumer<BleClient>(
      builder: (_, client, __) {
        if (client.connected) {
          return Card(
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Row(
                children: [
                  const Icon(Icons.check_circle, color: Colors.green),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Text(
                          'Connected',
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                        Text(
                          client.laptop?.platformName ?? 'Unknown device',
                          style: Theme.of(context).textTheme.bodySmall,
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          );
        }

        return Card(
          child: Padding(
            padding: const EdgeInsets.all(16.0),
            child: Row(
              children: [
                const Icon(Icons.bluetooth_disabled, color: Colors.grey),
                const SizedBox(width: 12),
                const Expanded(
                  child: Text(
                    'Not connected. Pair with your laptop to get started.',
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
