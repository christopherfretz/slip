class Node;

struct Base {
  virtual int get(int k) { return k; }
};

struct Node : Base {
  int v;
  int get(int k) { return v + k; }
};
