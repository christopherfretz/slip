// WP-104 (digest §9), consumed: __alignof__
int start(int) { return __alignof__(__func__) + sizeof(__func__); }
