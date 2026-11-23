template <class T> struct S { int f() { return 3; } int g() { return 1; } };
int u() { return S<int>().g() + S<int>().f(); }
