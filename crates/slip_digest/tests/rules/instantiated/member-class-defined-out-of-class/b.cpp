template <class T> struct S { struct J; };
template <class T> struct S<T>::J { int x = 1; };
int u() { return typename S<int>::J().x; }
