int order = 0;
struct A {
  int a = ++order * 10;
};
struct B {
  int b = ++order;
};
struct D : B, A {};

int first() { D d; return d.a; }
