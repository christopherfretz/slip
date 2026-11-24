// WP-112 (digest §9): an edit that changes no name-derived string: f's body changes beside its passed-on string, g changes; their names digests are equal
void take(const char *);
int f(int x) { take(__PRETTY_FUNCTION__); return x + 1; }
int g(int x) { take(__func__); return x; }
