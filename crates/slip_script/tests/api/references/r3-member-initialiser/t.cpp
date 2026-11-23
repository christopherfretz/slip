namespace N {
struct B {
  B(int);
};
struct D : B {
  int m;
  D() : B(1), m(2) {}
};
}
