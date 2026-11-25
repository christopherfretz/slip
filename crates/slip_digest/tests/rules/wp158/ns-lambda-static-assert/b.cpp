struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
auto gl = [](auto u) { return u.mf(1L); };
static_assert(sizeof(decltype(gl(X()))) == sizeof(int));
int w() { return 0; }
