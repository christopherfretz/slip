struct Base {
  int y() { return 1; }
};

struct D : Base {
  int y() { return 2; }
  int use() { return Base::y() + y(); }
};
