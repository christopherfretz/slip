struct X { static int f(int) { return 1; } static int f(long) { return 2; } int mf(int) { return 1; } int mf(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
int w() { auto l = [](auto u, int k = decltype(u)::f(1L)) { return k; }; return l(X()); }
