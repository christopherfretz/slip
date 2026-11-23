// WP-104 (digest §9), consumed: __alignof__
int f(int) { return __alignof__(__func__) + sizeof(__func__); }
