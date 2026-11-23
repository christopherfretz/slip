// WP-104 (digest §9), consumed: __typeof__
int start(int) { __typeof__(__func__) x = {}; return x[0]; }
