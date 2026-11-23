template <class T> struct S { template <class U> int k(U); };
template <class T> template <class U> int S<T>::k(U) { return 1; }
int u() { return S<int>().k(1); }
