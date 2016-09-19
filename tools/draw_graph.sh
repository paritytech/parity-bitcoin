#!/bin/bash

cargo graph --build-shape box --build-line-style dashed > graph.dot
dot -Tpng > graph.png graph.dot
