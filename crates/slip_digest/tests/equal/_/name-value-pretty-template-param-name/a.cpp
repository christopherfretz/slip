// WP-104 (digest §9), F2: what __PRETTY_FUNCTION__ prints beyond names (template-param-name)
template <class T> const char *f(T) { return __PRETTY_FUNCTION__; }
template const char *f<int>(int);
