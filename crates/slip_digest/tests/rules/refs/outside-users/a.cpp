namespace acme {
  struct S { int v; int get() const { return v; } };
  enum E { e0, e1 };
  enum class F { f0, f1 };
  int f(int x) { return x + 1; }
  int f(S s) { return s.v; }
  template<class T> T id(T t) { return t; }
  template<class T> struct Box { T t; };
  int var = 3;
}
struct Out { int w; };
int user() {
  acme::S s{1};
  int (*pf)(int) = &acme::f;
  int acme::S::*pm = &acme::S::v;
  acme::Box<acme::S> b{s};
  acme::Box<Out> o{{2}};
  acme::E e = acme::e1;
  acme::F g = acme::F::f1;
  return acme::f(1) + f(s) + pf(2) + s.*pm + s.get() + acme::id(3) + acme::id<Out>(o.t).w
       + b.t.v + e + (int)g + acme::var + sizeof(acme::Box<int>);
}
