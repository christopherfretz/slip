namespace { }
template <class T> struct S { int f() { return 1; } }; template <> struct S<int> { int f() { return 2; } };
int u() { return S<int>().f() + S<char>().f(); }
