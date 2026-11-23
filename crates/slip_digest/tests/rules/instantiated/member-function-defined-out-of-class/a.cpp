template <class T> struct S { int f(); };
template <class T> int S<T>::f() { return 1; }
int u() { return S<int>().f(); }
