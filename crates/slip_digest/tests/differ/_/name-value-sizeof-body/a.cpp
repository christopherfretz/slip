// WP-104 (digest §9), consumed: sizeof(__func__) in a function body
int f(int) { return sizeof(__func__); }
