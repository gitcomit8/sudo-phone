/// phone_auth_app — Android phone app for BLE-based biometric sudo.
///
/// The app:
/// 1. Pairs with the laptop daemon via BLE (exchanges P-256 keys)
/// 2. Receives auth requests from the daemon
/// 3. Shows a biometric prompt when an auth request arrives
/// 4. Signs the response with the pairing key and sends it back

import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_blue_plus/flutter_blue_plus.dart';

import 'ble/ble_client.dart';
import 'auth_handler.dart';
import 'ui/home_screen.dart';

void main() {
  runApp(const PhoneAuthApp());
}

class PhoneAuthApp extends StatelessWidget {
  const PhoneAuthApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => BleClient()),
        ChangeNotifierProvider(create: (_) => AuthHandler()),
      ],
      child: MaterialApp(
        title: 'Phone Auth',
        theme: ThemeData(
          colorSchemeSeed: Colors.blue,
          useMaterial3: true,
        ),
        home: const HomeScreen(),
      ),
    );
  }
}
