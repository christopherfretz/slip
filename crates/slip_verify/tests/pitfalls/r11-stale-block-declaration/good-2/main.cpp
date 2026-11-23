struct X;
int assist(X &);
struct X {
  friend int assist(X &);
private:
  int s = 1;
};
int assist(X &x) { return x.s; }

int caller() { X x; return assist(x); }
