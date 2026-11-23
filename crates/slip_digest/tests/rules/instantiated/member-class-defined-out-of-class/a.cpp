template <class T> struct S { struct I; };
template <class T> struct S<T>::I { int x = 1; };
int u() { return typename S<int>::I().x; }
