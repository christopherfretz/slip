struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
int w() { struct L { int g(X x) { auto i = [](auto u) { return u.mf(1L); }; int (*p)(X) = i; return p(x); } }; return L().g(X()); }
