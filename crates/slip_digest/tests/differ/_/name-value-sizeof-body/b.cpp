// WP-104 (digest §9), consumed: sizeof(__func__) in a function body
int start(int) { return sizeof(__func__); }
