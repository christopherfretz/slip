namespace a {
template <class T> struct S {
  int f();
  int k() { return f(); }
};
template <class T> int S<T>::f() { return sizeof(T); }
template <> int S<char>::f() { return 7; }
}
int u() {
  a::S<int> s;
  a::S<char> c;
  return s.f() + s.k() + c.f();
}
