namespace n {
int f(int);
int f(double);
struct B {
  virtual int g();
};
struct D : B {
  int g() override;
};
int use(D& d, B& b) {
  return f(1) + f(2.0) + d.g() + b.g() + d.B::g();
}
}
