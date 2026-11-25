struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
template <class T> requires (sizeof([](auto u) { return u.mf(1L); }(T())) > 0) int t(T) { return 0; }
int w() { return t(X()); }
