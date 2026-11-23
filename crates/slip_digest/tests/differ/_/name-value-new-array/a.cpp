// WP-104 (digest §9), consumed: new char[sizeof __func__]
char *f(int) { return new char[sizeof(__func__)]; }
