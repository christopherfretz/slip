template <class T> struct S { static int v; };
template <class T> int S<T>::v = 1;
int u() { return S<int>::v; }
