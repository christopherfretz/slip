namespace a {
template <class T> struct S {
  int g();
  int k() { return g(); }
};
template <class T> int S<T>::g() { return sizeof(T); }
template <> int S<char>::g() { return 7; }
}
int u() {
  a::S<int> s;
  a::S<char> c;
  return s.g() + s.k() + c.g();
}
