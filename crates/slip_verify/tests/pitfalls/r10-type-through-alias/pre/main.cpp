struct Widget {
  int v;
};
using Handle = Widget;

int read(Handle h) { return h.v; }
