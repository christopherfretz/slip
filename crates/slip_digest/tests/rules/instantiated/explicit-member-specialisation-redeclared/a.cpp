template <class T> struct S { int f() { return 1; } };
template <> int S<int>::f();
int u() { return S<int>().f(); }
template <> int S<int>::f() { return 2; }
