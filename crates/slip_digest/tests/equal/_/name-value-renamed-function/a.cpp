// WP-104 (digest §9), __func__ of a renamed function, decayed
void take(const char *);
const char *f(int) { take(__func__); return __FUNCTION__; }
