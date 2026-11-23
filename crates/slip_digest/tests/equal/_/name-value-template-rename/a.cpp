// WP-104 (digest §9), review "not checked": a template's __func__ under a rename of the template
void take(const char *);
template <class T> void f(T) { take(__func__); take(__PRETTY_FUNCTION__); }
template void f<int>(int);
