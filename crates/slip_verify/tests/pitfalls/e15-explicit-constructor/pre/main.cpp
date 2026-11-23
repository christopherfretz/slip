struct Y {};

struct T {
  T(const Y &) : k(1) {}
  explicit T(Y &) : k(2) {}
  int k;
};

int make(Y &y) {
  T x = y;
  return x.k;
}
