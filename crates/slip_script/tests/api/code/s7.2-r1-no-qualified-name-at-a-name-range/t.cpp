template <class T>
struct W {
  void f();
  void g();
  static void k();
  static void h();
};
template <>
struct W<char> {
  int f();
  int g();
};
struct A {
  union {
    int u;
    int v;
  };
};
struct Fr {
  friend void only_friend(Fr);
  friend void other_friend(Fr);
};
void take(int, int);
void use(W<int> w, W<long> l, W<char> c, A a, Fr fr) {
  w.f();
  l.f();
  take(c.f(), c.g());
  w.g();
  a.u = 1;
  only_friend(fr);
  W<int>::k();
}
