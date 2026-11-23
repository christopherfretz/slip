namespace N {
struct S {
  int m;
  int f();
};
int h(S a, S* p) { return a.m + p->f(); }
}
