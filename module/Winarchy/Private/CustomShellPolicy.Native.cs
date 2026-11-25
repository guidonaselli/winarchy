using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

namespace Winarchy
{
    public sealed class CustomShellPolicyValue
    {
        public bool Present { get; set; }
        public uint Kind { get; set; }
        public byte[] RawBytes { get; set; }
    }

    public sealed class CustomShellPolicyMutation
    {
        public bool Applied { get; set; }
        public bool Changed { get; set; }
        public CustomShellPolicyValue Current { get; set; }
    }

    public static class CustomShellPolicyNative
    {
        private const int ErrorFileNotFound = 2;
        private const int ErrorMoreData = 234;
        private const int KeyQueryValue = 0x0001;
        private const int KeySetValue = 0x0002;
        private const int MaximumValueSize = 1024 * 1024;
        private const uint RegSz = 1;

        [DllImport("Advapi32.dll", SetLastError = true)]
        private static extern int RegOpenCurrentUser(uint access, out IntPtr key);

        [DllImport("Advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "RegOpenKeyExW")]
        private static extern int RegOpenKey(IntPtr key, string subKey, uint options, uint access, out IntPtr result);

        [DllImport("Advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "RegOpenKeyTransactedW")]
        private static extern int RegOpenKeyTransacted(IntPtr key, string subKey, uint options, uint access, out IntPtr result, IntPtr transaction, IntPtr extendedParameter);

        [DllImport("Advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "RegCreateKeyTransactedW")]
        private static extern int RegCreateKeyTransacted(IntPtr key, string subKey, uint reserved, string keyClass, uint options, uint access, IntPtr securityAttributes, out IntPtr result, out uint disposition, IntPtr transaction, IntPtr extendedParameter);

        [DllImport("Advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "RegQueryValueExW")]
        private static extern int RegQueryValue(IntPtr key, string name, IntPtr reserved, out uint kind, byte[] data, ref uint dataLength);

        [DllImport("Advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "RegSetValueExW")]
        private static extern int RegSetValue(IntPtr key, string name, uint reserved, uint kind, byte[] data, uint dataLength);

        [DllImport("Advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "RegDeleteValueW")]
        private static extern int RegDeleteValue(IntPtr key, string name);

        [DllImport("Advapi32.dll", SetLastError = true)]
        private static extern int RegCloseKey(IntPtr key);

        [DllImport("KtmW32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateTransaction(IntPtr attributes, IntPtr unitOfWork, uint createOptions, uint isolationLevel, uint isolationFlags, uint timeout, string description);

        [DllImport("KtmW32.dll", SetLastError = true)]
        private static extern bool CommitTransaction(IntPtr transaction);

        [DllImport("KtmW32.dll", SetLastError = true)]
        private static extern bool RollbackTransaction(IntPtr transaction);

        [DllImport("Kernel32.dll", SetLastError = true)]
        private static extern bool CloseHandle(IntPtr handle);

        public static CustomShellPolicyValue ReadCurrent(string subKey, string valueName)
        {
            IntPtr root = IntPtr.Zero;
            IntPtr key = IntPtr.Zero;
            try
            {
                Check(RegOpenCurrentUser(KeyQueryValue, out root));
                int status = RegOpenKey(root, subKey, 0, KeyQueryValue, out key);
                if (status == ErrorFileNotFound) return Absent();
                Check(status);
                return ReadValue(key, valueName);
            }
            finally
            {
                if (key != IntPtr.Zero) RegCloseKey(key);
                if (root != IntPtr.Zero) RegCloseKey(root);
            }
        }

        public static CustomShellPolicyMutation CompareAndWrite(string subKey, string valueName,
            bool expectedPresent, uint expectedKind, byte[] expectedBytes,
            bool desiredPresent, uint desiredKind, byte[] desiredBytes)
        {
            IntPtr root = IntPtr.Zero;
            IntPtr key = IntPtr.Zero;
            IntPtr transaction = IntPtr.Zero;
            bool finished = false;
            try
            {
                Check(RegOpenCurrentUser(KeyQueryValue | KeySetValue, out root));
                transaction = CreateTransaction(IntPtr.Zero, IntPtr.Zero, 0, 0, 0, 0, "Winarchy CustomShell policy update");
                if (transaction == IntPtr.Zero || transaction == new IntPtr(-1)) ThrowLastError();

                int status = RegOpenKeyTransacted(root, subKey, 0, KeyQueryValue | KeySetValue, out key, transaction, IntPtr.Zero);
                if (status == ErrorFileNotFound)
                {
                    uint disposition;
                    Check(RegCreateKeyTransacted(root, subKey, 0, null, 0, KeyQueryValue | KeySetValue,
                        IntPtr.Zero, out key, out disposition, transaction, IntPtr.Zero));
                }
                else Check(status);

                CustomShellPolicyValue current = ReadValue(key, valueName);
                if (!Equal(current, expectedPresent, expectedKind, expectedBytes))
                {
                    RollbackTransaction(transaction);
                    finished = true;
                    return new CustomShellPolicyMutation { Applied = false, Changed = false, Current = current };
                }
                if (Equal(current, desiredPresent, desiredKind, desiredBytes))
                {
                    RollbackTransaction(transaction);
                    finished = true;
                    return new CustomShellPolicyMutation { Applied = true, Changed = false, Current = current };
                }

                if (desiredPresent)
                {
                    byte[] data = desiredBytes ?? new byte[0];
                    Check(RegSetValue(key, valueName, 0, desiredKind, data, (uint)data.Length));
                }
                else Check(RegDeleteValue(key, valueName));

                RegCloseKey(key);
                key = IntPtr.Zero;
                if (!CommitTransaction(transaction))
                {
                    RollbackTransaction(transaction);
                    finished = true;
                    CustomShellPolicyValue afterFailure = ReadCurrent(subKey, valueName);
                    return new CustomShellPolicyMutation { Applied = false, Changed = false, Current = afterFailure };
                }
                finished = true;
                CustomShellPolicyValue afterCommit = ReadCurrent(subKey, valueName);
                bool applied = Equal(afterCommit, desiredPresent, desiredKind, desiredBytes);
                return new CustomShellPolicyMutation { Applied = applied, Changed = applied, Current = afterCommit };
            }
            finally
            {
                if (key != IntPtr.Zero) RegCloseKey(key);
                if (!finished && transaction != IntPtr.Zero && transaction != new IntPtr(-1)) RollbackTransaction(transaction);
                if (transaction != IntPtr.Zero && transaction != new IntPtr(-1)) CloseHandle(transaction);
                if (root != IntPtr.Zero) RegCloseKey(root);
            }
        }

        private static CustomShellPolicyValue ReadValue(IntPtr key, string valueName)
        {
            for (int attempt = 0; attempt < 3; attempt++)
            {
                uint kind;
                uint length = 0;
                int status = RegQueryValue(key, valueName, IntPtr.Zero, out kind, null, ref length);
                if (status == ErrorFileNotFound) return Absent();
                if (status != 0 && status != ErrorMoreData) Check(status);
                if (length > MaximumValueSize) throw new InvalidOperationException("CustomShell registry value is too large to snapshot safely.");
                byte[] data = new byte[length];
                uint actualLength = length;
                status = RegQueryValue(key, valueName, IntPtr.Zero, out kind, data, ref actualLength);
                if (status == ErrorMoreData) continue;
                Check(status);
                if (actualLength > data.Length) continue;
                if (actualLength != data.Length) Array.Resize(ref data, (int)actualLength);
                return new CustomShellPolicyValue { Present = true, Kind = kind, RawBytes = data };
            }
            throw new InvalidOperationException("CustomShell registry value changed repeatedly while being read.");
        }

        private static CustomShellPolicyValue Absent()
        {
            return new CustomShellPolicyValue { Present = false, Kind = 0, RawBytes = new byte[0] };
        }

        private static bool Equal(CustomShellPolicyValue value, bool present, uint kind, byte[] bytes)
        {
            if (value.Present != present) return false;
            if (!present) return true;
            byte[] expected = bytes ?? new byte[0];
            if (value.Kind != kind || value.RawBytes.Length != expected.Length) return false;
            for (int index = 0; index < expected.Length; index++)
                if (value.RawBytes[index] != expected[index]) return false;
            return true;
        }

        private static void Check(int status)
        {
            if (status != 0) throw new Win32Exception(status);
        }

        private static void ThrowLastError()
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
    }
}
