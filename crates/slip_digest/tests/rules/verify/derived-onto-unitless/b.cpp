template <class T> struct S { int f() { return 1; } };
int u() { return S<int>().f(); }
template <class T> int t() { extern int hidden(); return hidden(); }
