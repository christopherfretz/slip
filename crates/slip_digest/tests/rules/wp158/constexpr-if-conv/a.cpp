struct X { static int f(int) { return 1; } static int f(long) { return 2; } int mf(int) { return 1; } int mf(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
template <class T> int t(T x) { auto l = [](auto u) { return u.mf(1L); }; if constexpr (sizeof(T) > 0) { int (*p)(T) = l; return p(x); } else { return 0; } }
int w() { return t(X()); }
