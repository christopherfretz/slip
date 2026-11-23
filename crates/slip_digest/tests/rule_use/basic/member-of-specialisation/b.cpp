template <class T> struct S { int f() { return 1; } };
template <> struct S<int> { int g() { return 2; } };
int u() { return S<int>().g() + S<long>().f(); }
