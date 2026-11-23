// WP-104 (digest §9), an immediate invocation's stored value points to the name (a constant, written as which builtin)
consteval const char *id(const char *p) { return p; }
consteval const void *vid(const void *p) { return p; }
void take(const char *);
void takev(const void *);
void f(int) { take(id(__func__)); takev(vid(__PRETTY_FUNCTION__)); take(id(__PRETTY_FUNCTION__ + 1)); }
