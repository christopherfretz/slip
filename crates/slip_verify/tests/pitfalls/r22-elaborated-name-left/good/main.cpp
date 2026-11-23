struct New {
  int v;
};
int take(struct New *) { return 1; }
int take(void *) { return 2; }

int caller() {
  New o{0};
  return take(&o);
}
