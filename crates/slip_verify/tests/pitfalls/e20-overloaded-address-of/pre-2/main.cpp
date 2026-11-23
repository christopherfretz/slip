struct Odd {
  int v;
  Odd *operator&();
};
Odd spare{7};
Odd *Odd::operator&() { return __builtin_addressof(spare); }
template <class T> T *addr(T &r) { return __builtin_addressof(r); }

int read(Odd &o) { return (&o)->v; }
