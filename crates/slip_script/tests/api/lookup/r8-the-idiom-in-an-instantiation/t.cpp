template <class T>
struct W {
  void f();
  void g();
};
template <class T>
struct O {
  struct I {
    void f();
    void g();
  };
};
template <class T>
struct X {
  void f();
  void g();
};
template struct X<char>;
void use(W<int> w, O<int>::I i, X<char> x) {
  w.f();
  i.f();
  x.f();
}
