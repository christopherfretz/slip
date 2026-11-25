struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
template <class T> struct S { int (*p)(T); S() : p([](auto u) { return u.mf(1L); }) {} };
int w() { return S<X>().p(X()); }
