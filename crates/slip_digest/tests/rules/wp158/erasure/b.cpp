struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
struct E { int (*f)(void*, X); void* o; template <class L> E(L& l) : f([](void* o, X x) { return (*(L*)o)(x); }), o(&l) {} };
int w() { auto l = [](auto u) { return u.mf(1L); }; E e(l); return e.f(e.o, X()); }
