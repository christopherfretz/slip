// WP-104 (digest §9), consumed: decltype(auto) of (__func__)
int f(int) { decltype(auto) r = (__func__); return sizeof r; }
