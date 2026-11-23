// WP-104 (digest §9), consumed: a type trait over decltype(__func__)
bool f(int) { return __is_same(decltype(__func__), const char[2]); }
