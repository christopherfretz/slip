struct S {
  union {
    int a;
    struct {
      int b;
    };
  };
  struct {
    int c;
  };
};
static union {
  int g;
};
