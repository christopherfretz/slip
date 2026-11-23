struct X {
  friend int assist(X &x) { return x.s; }
private:
  int s = 1;
};
int assist(X &);

int caller() { X x; return assist(x); }
