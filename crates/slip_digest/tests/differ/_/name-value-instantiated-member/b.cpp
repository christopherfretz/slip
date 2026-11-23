// WP-045 with WP-104 (digest §9): __func__ against __FUNCTION__ in an implicitly instantiated member; the change is in S<int>::m()'s <definition>
template <class T> struct S { const char *m() { return __FUNCTION__; } };
const char *f() { return S<int>().m(); }
