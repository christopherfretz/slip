template <class T>
struct S {
  void g() {}
  static int k;
};
template <class T>
int S<T>::k = 0;
int use() {
  S<long> s;
  s.g();
  return S<int>::k;
}
