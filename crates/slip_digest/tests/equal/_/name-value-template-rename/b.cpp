// WP-104 (digest §9), review "not checked": a template's __func__ under a rename of the template
void take(const char *);
template <class T> void start(T) { take(__func__); take(__PRETTY_FUNCTION__); }
template void start<int>(int);
