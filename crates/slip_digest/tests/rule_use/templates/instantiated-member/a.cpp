template <class T> struct S { int f() { return 1; } };
int u() { return S<int>().f() + S<long>().f(); }
