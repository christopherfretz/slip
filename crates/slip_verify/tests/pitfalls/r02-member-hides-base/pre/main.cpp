struct Base {
  int y() { return 1; }
};

struct D : Base {
  int x() { return 2; }
  int use() { return y() + x(); }
};
