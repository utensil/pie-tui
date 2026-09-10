import * as adapter from '../index.js'
import { registerAutocompleteTests } from './editor-autocomplete-scenarios.mjs'

registerAutocompleteTests(adapter, 'native facade')
