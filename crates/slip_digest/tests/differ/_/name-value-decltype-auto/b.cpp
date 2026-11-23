// WP-104 (digest §9), consumed: decltype(auto) of (__func__)
int start(int) { decltype(auto) r = (__func__); return sizeof r; }
