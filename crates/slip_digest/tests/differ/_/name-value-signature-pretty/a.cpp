// WP-104 (digest §9), __PRETTY_FUNCTION__ in two functions of one name and different written signatures
const char *f(int) { return __PRETTY_FUNCTION__; }
