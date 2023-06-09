#!/bin/bash

case "$1" in
  list)    
      echo -e rolldice'\t'" "
      exit 0
      ;;
  prepare)    
      exit 0
      ;;    
  update-list)   
      sleep 5         
      exit 0
      ;;
  finalize)      
      sleep 5         
      exit 0
      ;;        
  *)     
     exit 0
     ;;
  esac
         
