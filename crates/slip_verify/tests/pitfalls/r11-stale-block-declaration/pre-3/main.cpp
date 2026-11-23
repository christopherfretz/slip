struct X {
  friend int helper(X &x) { return x.s; }
private:
  int s = 1;
};
int helper(X &);

int caller() { X x; return helper(x); }
