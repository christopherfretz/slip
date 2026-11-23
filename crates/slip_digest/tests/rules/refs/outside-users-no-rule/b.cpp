namespace legacy {
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
  legacy::S s{1};
  int (*pf)(int) = &legacy::f;
  int legacy::S::*pm = &legacy::S::v;
  legacy::Box<legacy::S> b{s};
  legacy::Box<Out> o{{2}};
  legacy::E e = legacy::e1;
  legacy::F g = legacy::F::f1;
  return legacy::f(1) + f(s) + pf(2) + s.*pm + s.get() + legacy::id(3) + legacy::id<Out>(o.t).w
       + b.t.v + e + (int)g + legacy::var + sizeof(legacy::Box<int>);
}
