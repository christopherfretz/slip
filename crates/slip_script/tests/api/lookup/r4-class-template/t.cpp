template <class T>
struct C {
  int m;
  void f();
  struct In {
    T k;
  };
};
template <class T>
void C<T>::f() {}
template <>
struct C<int> {
  int q;
};
