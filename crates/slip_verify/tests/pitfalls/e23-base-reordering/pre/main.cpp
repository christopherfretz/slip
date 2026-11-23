int order = 0;
struct A {
  int a = ++order * 10;
};
struct B {
  int b = ++order;
};
struct D : A, B {};

int first() { D d; return d.a; }
