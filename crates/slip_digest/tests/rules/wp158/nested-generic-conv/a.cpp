struct X { static int f(int) { return 1; } static int f(long) { return 2; } int mf(int) { return 1; } int mf(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
int w() { auto o = [](auto v) { auto i = [](auto u) { return u.mf(1L); }; int (*p)(decltype(v)) = i; return p(v); }; return o(X()); }
